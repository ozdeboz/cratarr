// greet-by-time (MIT, Home's personal greeting): its lists of greetings per part of the day; it ships no types.
declare module 'greet-by-time' {
  const greet: ((name: string, hour: number) => string) & {
    morningGreetings: string[];
    dayGreetings: string[];
    eveningGreetings: string[];
  };
  export default greet;
}
